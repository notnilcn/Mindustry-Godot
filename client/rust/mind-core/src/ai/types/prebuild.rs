// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PrebuildAI` (plan 11 §4.2, upstream-labeled **experimental**).
//!
//! Ported from `core/src/mindustry/ai/types/PrebuildAI.java`. The wave
//! pre-build controller walks the unit toward an assigned destination before the
//! wave is released; the port keeps the state and the deterministic path
//! behavior behind a rustdoc `experimental` note and excludes it from the
//! `units_mid` budget until proven hot (plan 11 §8 R11).

use bevy_ecs::entity::Entity;

use crate::world::TilePos;

use super::super::ai_controller::AiCtx;

/// `PrebuildAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrebuildAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Pre-build destination tile.
    pub target: Option<TilePos>,
    /// Whether the unit has reached the pre-build spot.
    pub arrived: bool,
}

/// `PrebuildAI.updateUnit` (experimental): path toward the pre-build spot.
pub fn update_prebuild(ctx: &mut AiCtx, unit: Entity, state: &mut PrebuildAi) -> bool {
    let Some(target) = state.target else {
        return true;
    };
    let arrive = ctx
        .world
        .get::<crate::entities::comp::unit::HitboxComp>(unit)
        .map(|hitbox| hitbox.hit_size.max(4.0))
        .unwrap_or(4.0);
    state.arrived = ctx.pathfind(unit, target, arrive);
    state.arrived
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn prebuild_walks_to_destination() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let unit = harness.spawn("dagger", 0, 44.0, 44.0, 0.0).expect("dagger");
        let mut state = PrebuildAi {
            unit: Some(unit),
            target: Some(TilePos::new(60, 60)),
            arrived: false,
        };
        let mut arrived = false;
        for _ in 0..1500 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            if update_prebuild(&mut ctx, unit, &mut state) {
                arrived = true;
                break;
            }
        }
        assert!(arrived);
    }
}
