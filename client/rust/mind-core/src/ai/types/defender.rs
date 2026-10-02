// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DefenderAI` (plan 11 §4.2). A stationary controller that acquires the
//! nearest hostile in range, faces it and opens fire. Ported from
//! `core/src/mindustry/ai/types/DefenderAI.java` (`updateUnit` targets, no
//! movement).

use bevy_ecs::entity::Entity;

use super::super::ai_controller::AiCtx;

/// `DefenderAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefenderAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current target.
    pub target: Option<Entity>,
}

/// `DefenderAI.updateUnit`: acquire/fire at the nearest hostile.
///
/// Returns whether the unit has a target this tick.
pub fn update_defender(ctx: &mut AiCtx, unit: Entity, state: &mut DefenderAi) -> bool {
    let (range, target_air, target_ground) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| (def.range, def.target_air, def.target_ground))
        .unwrap_or((150.0, true, true));
    let target = ctx.find_target(unit, range, target_air, target_ground);
    state.target = target;
    match target {
        Some(target) => {
            ctx.target(unit, Some(target));
            true
        }
        None => {
            ctx.stop_shooting(unit);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;
    use crate::entities::comp::unit::weapon_mount::WeaponsComp;

    #[test]
    fn defender_fires_without_moving() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let a = harness.spawn("dagger", 0, 100.0, 100.0, 0.0).expect("a");
        let b = harness.spawn("dagger", 1, 160.0, 100.0, 0.0).expect("b");
        assert_ne!(a, b);
        let mut state = DefenderAi {
            unit: Some(a),
            target: None,
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        assert!(update_defender(&mut ctx, a, &mut state));
        let weapons = harness.build.world.get::<WeaponsComp>(a).unwrap();
        assert!(weapons.mounts.iter().all(|mount| mount.shoot));
        // No movement was issued.
        let vel = harness
            .build
            .world
            .get::<crate::entities::comp::Vel>(a)
            .unwrap();
        assert_eq!((vel.x, vel.y), (0.0, 0.0));
    }
}
