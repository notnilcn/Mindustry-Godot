// SPDX-License-Identifier: GPL-3.0-only

//! Game phase state.
//!
//! Ported from `core/src/mindustry/core/GameState.java` (P0 subset: phase + tick).

use serde::{Deserialize, Serialize};

/// The coarse game phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// In a menu / not simulating.
    #[default]
    Menu,
    /// A game is running.
    Playing,
    /// A game is loaded but the fixed-step pump is halted.
    Paused,
}

impl State {
    /// Stable byte used by the checksum stream (§6.4).
    pub const fn as_u8(self) -> u8 {
        match self {
            State::Menu => 0,
            State::Playing => 1,
            State::Paused => 2,
        }
    }

    /// Whether a world is loaded (`Playing` or `Paused`).
    pub const fn is_game(self) -> bool {
        matches!(self, State::Playing | State::Paused)
    }
}

/// Simulation state header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    /// Current phase.
    pub phase: State,
    /// Number of completed ticks.
    pub tick: u64,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            phase: State::Menu,
            tick: 0,
        }
    }
}

impl GameState {
    /// Creates a state at tick 0 in the given phase.
    pub fn new(phase: State) -> Self {
        Self { phase, tick: 0 }
    }

    /// Changes the phase, returning `true` when it actually changed.
    pub fn set_phase(&mut self, phase: State) -> bool {
        if self.phase == phase {
            false
        } else {
            self.phase = phase;
            true
        }
    }

    /// Advances the tick counter by one.
    pub fn advance(&mut self) {
        self.tick = self.tick.saturating_add(1);
    }
}
