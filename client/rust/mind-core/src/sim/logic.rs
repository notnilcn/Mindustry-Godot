// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Logic` phase systems (state-free).
//!
//! Ported from `core/src/mindustry/core/Logic.java`. This module owns the
//! state-free helper predicates `Logic.update()` consults; the full schedule
//! lives in `crate::schedule` (M6). `Logic` holds no long-lived state (plan 05
//! §3.12): all match state is in `GameState`/`Rules`/`Teams`.

use crate::game::{GameState, State};

/// Run condition: the game phase is loaded and not paused.
pub fn is_game_active(state: &GameState) -> bool {
    state.is_playing()
}

/// Run condition: the game phase is loaded (playing or paused).
pub fn is_game(state: &GameState) -> bool {
    state.is_game()
}

/// `Logic.checkGameState()` skeleton: returns the phase to transition to, if
/// any. Full win/lose logic is plan 12; this returns `None` (no transition).
pub fn check_game_state(_state: &GameState) -> Option<State> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_conditions_track_phase() {
        let mut state = GameState::new(State::Menu);
        assert!(!is_game_active(&state));
        assert!(!is_game(&state));
        state.set_phase(State::Playing);
        assert!(is_game_active(&state));
        assert!(is_game(&state));
        state.set_phase(State::Paused);
        assert!(!is_game_active(&state));
        assert!(is_game(&state));
    }

    #[test]
    fn check_game_state_is_a_noop_until_plan_12() {
        assert_eq!(check_game_state(&GameState::new(State::Playing)), None);
    }
}
