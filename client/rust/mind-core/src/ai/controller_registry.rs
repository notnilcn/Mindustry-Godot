// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Controller selection registry (plan 11 §3.5).
//!
//! Upstream `UnitType.controller` is a `Func<Unit, UnitController>`; the port
//! keeps a data [`AiKind`] plus this selection function. `CommandAI` is the
//! player/RTS controller; `select_ai` is the per-type AI preset. The full
//! expression is:
//!
//! ```text
//! !playerControllable || (team.isAI() && !team.rules().rtsAi) ? aiController : CommandAI
//! ```
//!
//! Team/rules flags come from plan 12; callers supply `team_is_ai`/`rts_ai` so
//! this module stays free of `TeamData`.

use crate::content::registries::units::UnitTypeDef;

use super::controller::{AiKind, select_ai};

/// Selects the controller for a unit given the owning team's AI flags.
///
/// `player_controlled` is `true` when plan 15 has possessed the unit; that path
/// installs the `Player` bridge (plan 15 owns its body).
pub fn select_controller(
    unit: &UnitTypeDef,
    team_is_ai: bool,
    rts_ai: bool,
    player_controlled: bool,
) -> AiKind {
    if player_controlled {
        return AiKind::Player;
    }
    if !unit.player_controllable || (team_is_ai && !rts_ai) {
        select_ai(unit)
    } else {
        AiKind::Command
    }
}

/// Whether a controller survives save/load (`keepState`).
///
/// Vanilla `LogicAI` and `CommandAI` keep their state; every other controller
/// resets to a fresh instance.
pub const fn keep_state(kind: AiKind) -> bool {
    matches!(kind, AiKind::Logic | AiKind::Command)
}

/// Whether plan 13's `LUnitControl` may install a `LogicAI` on the unit.
///
/// Mirrors the `UnitType.logicControllable` gate plus the `NoAI` override.
pub const fn is_logic_controllable(unit: &UnitTypeDef) -> bool {
    unit.logic_controllable
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore};

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn selection_matrix_matches_unit_type_semantics() {
        let content = content();
        // Player-controllable AI unit on an AI team -> its AI preset.
        let dagger = content.unit_by_name("dagger").expect("dagger");
        assert_eq!(
            select_controller(dagger, true, false, false),
            AiKind::Ground
        );
        // Same type on an RTS-AI team -> `CommandAI`.
        assert_eq!(
            select_controller(dagger, true, true, false),
            AiKind::Command
        );
        // Possessed -> `Player`.
        assert_eq!(select_controller(dagger, true, true, true), AiKind::Player);
        // Non-player-controllable always gets its AI (`dummy` -> `NoAI`).
        let dummy = content.unit_by_name("dummy").expect("dummy");
        assert_eq!(select_controller(dummy, false, false, false), AiKind::NoAi);
        // Assembler controller.
        let assembly_drone = content
            .unit_by_name("assembly-drone")
            .expect("assembly-drone");
        assert_eq!(
            select_controller(assembly_drone, true, false, false),
            AiKind::Assembler
        );
    }

    #[test]
    fn keep_state_only_for_logic_and_command() {
        assert!(keep_state(AiKind::Logic));
        assert!(keep_state(AiKind::Command));
        assert!(!keep_state(AiKind::Ground));
        assert!(!keep_state(AiKind::Miner));
    }

    #[test]
    fn logic_controllable_matches_vanilla() {
        let content = content();
        assert!(is_logic_controllable(
            content.unit_by_name("dagger").expect("dagger")
        ));
        // The target dummy is not logic-controllable.
        assert!(!is_logic_controllable(
            content.unit_by_name("dummy").expect("dummy")
        ));
    }
}
