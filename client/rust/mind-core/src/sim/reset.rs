// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Logic.play` / `Logic.reset` / pause flows (plan 05 M3).
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`play()`, `reset()`), and
//! `core/GameState.java` transitions. A full world reload is plan 06; here the
//! flows clear the P0 grid/entities and fire the canonical events.

use crate::content::BlockId;
use crate::event::{PlayEvent, ResetEvent, SimEvent, StateChangeEvent};
use crate::game::{GameState, State};

use super::events::Trigger;

impl super::Sim {
    /// Begins/resumes play: sets `Playing`, fires `StateChangeEvent`,
    /// `PlayEvent` and `Trigger.newGame` (port of `Logic.play`).
    pub fn play(&mut self) -> bool {
        let from = self.state.phase;
        if !self.state.set_phase(State::Playing) {
            return false;
        }
        self.push_event(SimEvent::StateChangeEvent(StateChangeEvent {
            from,
            to: State::Playing,
        }));
        self.push_event(SimEvent::PlayEvent(PlayEvent));
        self.triggers.fire(Trigger::NewGame, &mut self.ecs.0);
        true
    }

    /// Resets to the menu: clears entities/grid/delayed runs, resets the clock,
    /// fires `ResetEvent` (and `StateChangeEvent` when leaving a game); port of
    /// `Logic.reset` (`Groups.clear`, `Time.clear`, reset `GameState`).
    pub fn reset(&mut self) {
        let from = self.state.phase;
        self.ecs.0.clear_entities();
        self.grid.fill(BlockId::AIR, BlockId::AIR);
        self.time.clear();
        self.clock.reset();
        self.state = GameState::new(State::Menu);
        if from != State::Menu {
            self.push_event(SimEvent::StateChangeEvent(StateChangeEvent {
                from,
                to: State::Menu,
            }));
        }
        self.push_event(SimEvent::ResetEvent(ResetEvent));
    }

    /// Pauses a running game; returns whether the phase changed.
    pub fn pause(&mut self) -> bool {
        self.set_phase(State::Paused)
    }

    /// Resumes to `Playing`; returns whether the phase changed.
    pub fn resume(&mut self) -> bool {
        self.set_phase(State::Playing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::SimEvent;
    use crate::sim::Sim;

    #[test]
    fn reset_play_cycle() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        sim.set_phase(State::Menu);
        // play → playing, tick advances.
        assert!(sim.play());
        assert_eq!(sim.phase(), State::Playing);
        for _ in 0..5 {
            sim.tick().expect("tick");
        }
        assert_eq!(sim.tick_count(), 5);
        // pause/resume.
        assert!(sim.pause());
        assert!(sim.is_paused());
        assert!(sim.resume());
        assert_eq!(sim.phase(), State::Playing);
        // reset → menu, clock zeroed, reset event queued.
        sim.reset();
        assert_eq!(sim.phase(), State::Menu);
        assert_eq!(sim.clock().time, 0.0);
        assert_eq!(sim.clock().update_id, 0);
        let events = sim.pending_events.clone();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SimEvent::ResetEvent(_)))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SimEvent::StateChangeEvent(_)))
        );
    }
}
